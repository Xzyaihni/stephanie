(define list (lambda xs xs))

(define (fold f start xs)
    (if (null? xs)
        start
        (fold f (f (car xs) start) (cdr xs))))

(define (map f lst)
    (if (null? lst)
        '()
        (cons (f (car lst)) (map f (cdr lst)))))

(define (for-each f lst)
    (begin
        (map f lst)
        '()))
