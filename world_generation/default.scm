(define size-x 16)
(define size-y 16)

(define (filled-chunk tile)
    (make-vector (* size-x size-y) tile))

(define (index-of point)
    (+ (* size-x (point-y point)) (point-x point))) 

(define (make-point x y)
    (cons x y))

(define (point-x point)
    (car point))

(define (point-y point)
    (cdr point))

(define (make-area bl tr)
    (cons bl tr))

(define (area-bl area)
    (car area))

(define (area-tr area)
    (cdr area))

(define (vertical-line-length chunk x len tile)
    (if (= len 0)
        chunk
        (begin
            (vector-set!
                chunk
                (index-of (make-point x (- len 1)))
                tile)
            (vertical-line-length chunk x (- len 1) tile))))

(define (vertical-line chunk x tile)
    (vertical-line-length chunk x size-y tile))

(define (horizontal-line-length chunk y len tile)
    (if (= len 0)
        chunk
        (begin
            (vector-set!
                chunk
                (index-of (make-point (- len 1) y))
                tile)
            (horizontal-line-length chunk y (- len 1) tile))))

(define (horizontal-line chunk y tile)
    (horizontal-line-length chunk y size-x tile))

(define (fill-area chunk area tile)
    (let ((top (area-tr area)) (bottom (area-bl area)))
        (if (> (point-x bottom) (point-x top))
            chunk
            (begin
                (vertical-line-length
                    chunk
                    (point-x bottom)
                    (- (point-y top) (point-y bottom))
                    tile)
                (fill-area
                    chunk
                    (make-area
                        (make-point (+ 1 (point-x bottom)) (point-y bottom))
                        top)
                    tile)))))
