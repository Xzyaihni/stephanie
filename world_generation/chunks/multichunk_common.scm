(define (big-query this-chunk pos on-success on-fail)
    (let (
            (scaled-start (point-sub pos in-big-chunk-pos))
            (clip-check (lambda (v s) (and (> v -1) (< v s)))))
        (if (and
                (clip-check (point-x scaled-start) size-x)
                (clip-check (point-y scaled-start) size-y))
            (on-success scaled-start)
            (on-fail))))

(define (big-combine-markers this-chunk pos marker)
    (big-query
        this-chunk
        pos
        (lambda (scaled-start) (combine-markers this-chunk scaled-start marker))
        (lambda () this-chunk)))

;(define (big-get-tile this-chunk pos)
;    (big-query
;        this-chunk
;        pos
;        (lambda (scaled-start) (get-tile this-chunk scaled-start))
;        (lambda () '())))

(define (big-put-tile this-chunk pos fill-tile)
    (big-query
        this-chunk
        pos
        (lambda (scaled-start) (put-tile this-chunk scaled-start fill-tile))
        (lambda () this-chunk)))

(define (big-fill-area this-chunk area fill-tile)
    (let ((scaled-start (point-sub (area-start area) in-big-chunk-pos)))
        (let (
                (scaled-end (point-zip-map (point-add scaled-start (area-size area)) (make-point size-x size-y) (lambda (x y) (min x y))))
                (clipped-start (point-map scaled-start (lambda (x) (max x 0)))))
            (let ((clipped-size (point-sub scaled-end clipped-start)))
                (if (and
                        (and (> (point-x scaled-end) 0) (< (point-x scaled-start) size-x))
                        (and (> (point-y scaled-end) 0) (< (point-y scaled-start) size-y)))
                    (fill-area this-chunk (make-area clipped-start clipped-size) fill-tile)
                    this-chunk)))))

(define (big-horizontal-line this-chunk pos length fill-tile)
    (let ((scaled-start (point-sub pos in-big-chunk-pos)))
        (let (
                (scaled-end (point-zip-map (point-add scaled-start (make-point length 1)) (make-point size-x size-y) (lambda (x y) (min x y))))
                (clipped-start (point-map scaled-start (lambda (x) (max x 0)))))
            (let ((clipped-length (- (point-x scaled-end) (point-x clipped-start))))
                (if (and
                        (and (> (point-x scaled-end) 0) (< (point-x scaled-start) size-x))
                        (and (> (point-y scaled-end) 0) (< (point-y scaled-start) size-y)))
                    (horizontal-line-length this-chunk clipped-start clipped-length fill-tile)
                    this-chunk)))))

(define (big-vertical-line this-chunk pos length fill-tile)
    (let ((scaled-start (point-sub pos in-big-chunk-pos)))
        (let (
                (scaled-end (point-zip-map (point-add scaled-start (make-point 1 length)) (make-point size-x size-y) (lambda (x y) (min x y))))
                (clipped-start (point-map scaled-start (lambda (x) (max x 0)))))
            (let ((clipped-length (- (point-y scaled-end) (point-y clipped-start))))
                (if (and
                        (and (> (point-x scaled-end) 0) (< (point-x scaled-start) size-x))
                        (and (> (point-y scaled-end) 0) (< (point-y scaled-start) size-y)))
                    (vertical-line-length this-chunk clipped-start clipped-length fill-tile)
                    this-chunk)))))

(define (light-intensity x)
    (if (stop-between-difficulty 0.5 2.0) x (* x 0.2)))
